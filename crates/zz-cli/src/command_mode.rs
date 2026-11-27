use super::*;

const NATIVE_APP_USAGE: &str = "zz: usage: zz app";

const ALL_HOSTS: &str = "all";

pub(super) fn run_command_mode(
    arguments: &[RawText],
    socket_path: &Path,
    socket_source: SocketSelectionSource,
    host: Option<&str>,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
    shell_command: Option<&str>,
    login_shell: bool,
    startup_options: &StartupOptions,
    implicit_tmux_conflict: bool,
) -> Option<ExitCode> {
    if let Some(shell_command) = shell_command {
        if implicit_tmux_conflict && host.is_none() {
            eprintln!("{FOREIGN_TMUX_ERROR}");
            return Some(exit_code_for(CliFailure::Runtime));
        }
        return Some(run_tmux_shell_command(
            socket_path,
            socket_source,
            host,
            mux_config_files,
            no_start_server,
            shell_command,
            login_shell,
        ));
    }
    let mut command_chain = split_command_chain(arguments);
    if command_chain.is_empty() && host.is_some_and(|host| host != ALL_HOSTS) {
        command_chain = vec![CommandInvocation::new("new-session", ["-A"])];
    }
    if command_chain.is_empty()
        && host.is_none()
        && !bare_command_line_opens_application(
            startup_options.origin,
            launched_by_launch_services(),
        )
    {
        command_chain =
            default_client_command_chain(socket_path, mux_config_files, no_start_server);
    }
    let Some(invocation) = command_chain.first().cloned() else {
        if host.is_some() {
            eprintln!(
                "zz: --host all needs a read-only command, such as zz --host all list-panes -a"
            );
            return Some(exit_code_for(CliFailure::Usage));
        }
        return None;
    };
    let command = invocation.name.clone();
    if command == "--help" || command == "help" {
        print!("{}", top_level_help());
        return Some(ExitCode::SUCCESS);
    }
    if let Some(host) = host
        && host != ALL_HOSTS
        && matches!(
            canonical_command(&command),
            "fleet" | "list-hosts" | "tools" | "protocol-version" | "build-info" | "--version"
        )
    {
        return Some(run_host_cli(host, arguments));
    }
    if invocation
        .args
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--help")
    {
        return Some(match command_help(&command) {
            Ok(help) => {
                print!("{help}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", server_error_message(&error));
                exit_code_for(CliFailure::Server(&error))
            }
        });
    }
    if canonical_command(&command) == "tools" && host.is_none() && command_chain.len() == 1 {
        if let [flag, rest @ ..] = invocation.args.as_slice()
            && flag == "--agent-login"
        {
            return Some(agent_login::run(socket_path, rest));
        }
        return Some(agent_hooks::run(&invocation.args));
    }
    if command == "update" {
        if host == Some(ALL_HOSTS) {
            eprintln!("zz: zz update -a updates this machine and every host");
            return Some(exit_code_for(CliFailure::Usage));
        }
        let arguments = invocation
            .args
            .iter()
            .map(ToString::to_string)
            .chain(host.map(str::to_owned))
            .collect::<Vec<_>>();
        return Some(update::run(&arguments, socket_path));
    }
    if host == Some(ALL_HOSTS) {
        return Some(run_on_every_host(arguments, &command_chain));
    }
    if let Some(host) = host
        && let Some(message) = untargeted_host_input(host, &command_chain)
    {
        eprintln!("{message}");
        return Some(exit_code_for(CliFailure::Usage));
    }
    let progress_target = if command == "agent-send" {
        match events::progress_target(&invocation.args) {
            Ok(target) => target,
            Err(message) => {
                eprintln!("{message}");
                return Some(exit_code_for(CliFailure::Usage));
            }
        }
    } else {
        None
    };
    if command == "events" {
        if command_chain.len() != 1 {
            eprintln!("events: use a single command: zz events [-t target]");
            return Some(exit_code_for(CliFailure::Usage));
        }
        if let Some(host) = host {
            return Some(match configured_host_endpoint(host) {
                Ok(endpoint) => events::run_host(host, &endpoint, &invocation.args),
                Err(error) => {
                    eprintln!("zz: {error}");
                    exit_code_for(CliFailure::Runtime)
                }
            });
        }
        if implicit_tmux_conflict {
            eprintln!("{FOREIGN_TMUX_ERROR}");
            return Some(exit_code_for(CliFailure::Runtime));
        }
        return Some(events::run(
            socket_path,
            socket_source,
            mux_config_files,
            no_start_server,
            &invocation.args,
        ));
    }
    if command == "app" {
        if host.is_some() || !invocation.args.is_empty() || command_chain.len() != 1 {
            eprintln!("{NATIVE_APP_USAGE}");
            return Some(exit_code_for(CliFailure::Usage));
        }
        return None;
    }
    if is_version_command(&command) {
        println!("zz {}", env!("CARGO_PKG_VERSION"));
        return Some(ExitCode::SUCCESS);
    }
    if command == "protocol-version" {
        return Some(
            match protocol_version_output(
                invocation.args.iter().map(ToString::to_string),
                socket_source.is_overridden(),
            ) {
                Ok(version) => {
                    println!("{version}");
                    ExitCode::SUCCESS
                }
                Err(usage) => {
                    eprintln!("zz: {usage}");
                    exit_code_for(CliFailure::Usage)
                }
            },
        );
    }
    if command == "build-info" {
        if !invocation.args.is_empty() {
            eprintln!("zz: usage: zz build-info");
            return Some(exit_code_for(CliFailure::Usage));
        }
        println!("{}", zz_config::owned::BuildInfo::current());
        return Some(ExitCode::SUCCESS);
    }

    if host.is_some() && matches!(command.as_str(), "daemon" | "proxy") {
        eprintln!("zz: --host is not supported for `{command}`");
        return Some(exit_code_for(CliFailure::Usage));
    }

    if command == "daemon" {
        #[cfg(unix)]
        let handoff = match invocation.args.as_slice() {
            [flag, fd] if flag == DAEMON_START_LOCK_FD_ARGUMENT => fd.parse::<i32>().ok(),
            _ => None,
        };
        #[cfg(windows)]
        let handoff = matches!(
            invocation.args.as_slice(),
            [flag] if flag == DAEMON_RETRY_BIND_ARGUMENT
        );
        #[cfg(unix)]
        let handed_off = handoff.is_some();
        #[cfg(windows)]
        let handed_off = handoff;
        let bootstrap = if handed_off {
            DaemonBootstrapArguments::default()
        } else {
            match parse_daemon_bootstrap_arguments(&invocation.args) {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    eprintln!("zz daemon: {}", error.message());
                    return Some(exit_code_for(CliFailure::Usage));
                }
            }
        };
        #[cfg(not(windows))]
        purge_freed_memory_promptly();
        let mut daemon =
            Daemon::new(socket_path).with_mux_config_files(mux_config_files.iter().cloned());
        if let Some(server_id) = bootstrap.server_id {
            daemon = daemon.with_server_id(server_id);
        }
        if let Some(client_working_directory) = bootstrap.client_working_directory {
            daemon = daemon.with_initial_client_working_directory(client_working_directory);
        }
        #[cfg(unix)]
        if let Some(fd) = bootstrap.ready_fd {
            daemon = daemon.with_bootstrap_ready_fd(fd);
        }
        #[cfg(unix)]
        if let Some(fd) = handoff {
            daemon = daemon.with_start_lock_fd(fd);
        }
        #[cfg(windows)]
        if handoff {
            daemon = daemon.with_bind_retry();
        }
        return Some(match daemon.serve() {
            Ok(Stopped::Exit) | Err(DaemonError::AlreadyRunning(_)) => ExitCode::SUCCESS,
            Ok(Stopped::Restart(restart)) => {
                restart_daemon(socket_path, mux_config_files, &restart)
            }
            Err(error) => {
                eprintln!("zz daemon: {error}");
                exit_code_for(CliFailure::Runtime)
            }
        });
    }

    if command == "proxy" {
        return Some(match zz_daemon_client::run_socket_proxy(socket_path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("zz proxy: {error}");
                exit_code_for(CliFailure::Runtime)
            }
        });
    }

    if command == "tailnet" {
        return Some(
            match tailnet::run(invocation.args.iter().map(ToString::to_string), socket_path) {
                Ok(output) => {
                    println!("{output}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("zz: {error}");
                    exit_code_for(CliFailure::Usage)
                }
            },
        );
    }

    if command == "copy-file" {
        let words = invocation
            .args
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        return Some(match copy_file::run(&words) {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("copy-file: {error}");
                exit_code_for(if error.starts_with("usage:") {
                    CliFailure::Usage
                } else {
                    CliFailure::Runtime
                })
            }
        });
    }

    if command == "fleet" {
        return Some(
            match fleet::run(
                invocation
                    .args
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>(),
            ) {
                Ok(output) => {
                    if !output.is_empty() {
                        println!("{output}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    if !error.is_empty() {
                        eprintln!("zz: {error}");
                    }
                    exit_code_for(
                        if error.starts_with("usage:")
                            || error == "ssh destination must not start with `-`"
                        {
                            CliFailure::Usage
                        } else {
                            CliFailure::Runtime
                        },
                    )
                }
            },
        );
    }

    if command == "list-hosts" && host.is_none() {
        return Some(list_hosts::run(
            &invocation
                .args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        ));
    }

    if implicit_tmux_conflict && host.is_none() {
        eprintln!("{FOREIGN_TMUX_ERROR}");
        return Some(exit_code_for(CliFailure::Runtime));
    }

    if command == "--kill-server" {
        return Some(match host {
            Some(host) => run_host_kill_server(host, invocation.args),
            None => run_kill_server(socket_path, invocation.args),
        });
    }

    let start_server = !no_start_server && tmux_command_starts_server(&command);
    let routing = ChainRouting {
        socket_path,
        host,
        mux_config_files,
        startup_options,
        command: &command,
        start_server,
    };
    if let Some(host) = host {
        return Some(run_host_command_chain(
            &routing,
            host,
            socket_source,
            command_chain,
            progress_target.as_deref(),
        ));
    }

    let mut resume = None;
    let failure = match CommandClient::connect(socket_path) {
        Ok(client) => {
            match run_local_command_chain(
                &routing,
                client,
                &command_chain,
                progress_target.as_deref(),
                None,
            ) {
                LocalChain::Done(exit) => return Some(exit),
                LocalChain::Resume(prepared, client) => {
                    resume = Some((prepared, *client));
                    None
                }
                LocalChain::Unanswered(error) => Some(error),
            }
        }
        Err(error) => Some(error),
    };
    let failure = failure.map(|error| classify_local_connect_error(socket_path, error));

    if failure.is_some() {
        let static_commands = if matches!(command.as_str(), "attach" | "attach-session") {
            if let Err(error) = parse_native_attach_arguments(command_chain[0].args.clone()) {
                return Some(print_native_attach_argument_error(error));
            }
            &command_chain[1..]
        } else {
            &command_chain
        };
        if let Err(error) = zz_mux::validate_static_command_chain(static_commands) {
            eprintln!("{}", server_error_message(&error));
            return Some(exit_code_for(CliFailure::Server(&error)));
        }
    }
    let failure = match failure {
        Some(error)
            if !start_server
                && tmux_command_starts_server(&command)
                && daemon_is_spawnable(&error) =>
        {
            eprintln!("{}", format_local_command_error(socket_path, error));
            return Some(exit_code_for(CliFailure::Runtime));
        }
        failure => failure,
    };

    let failure = if start_server && failure.as_ref().is_some_and(daemon_is_spawnable) {
        if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
            eprintln!("{}", error.message);
            let nested_label_new_session =
                canonical_command(&command) == "new-session" && error.kind == ErrorKind::NotFound;
            return Some(if nested_label_new_session {
                ExitCode::SUCCESS
            } else {
                exit_code_for(CliFailure::Runtime)
            });
        }
        let client = match spawn_and_connect_command_client(socket_path, mux_config_files) {
            Ok(connected) => connected,
            Err(error) => {
                eprintln!("{}", format_local_command_error(socket_path, error));
                return Some(exit_code_for(CliFailure::Runtime));
            }
        };
        let (client, spawned_server_id) = client;
        match run_local_command_chain(
            &routing,
            client,
            &command_chain,
            progress_target.as_deref(),
            Some(spawned_server_id),
        ) {
            LocalChain::Done(exit) => return Some(exit),
            LocalChain::Resume(prepared, client) => {
                resume = Some((prepared, *client));
                None
            }
            LocalChain::Unanswered(error) => {
                eprintln!("{}", format_local_command_error(socket_path, error));
                return Some(exit_code_for(CliFailure::Runtime));
            }
        }
    } else {
        failure
    };

    if command == "kill-server" && resume.is_none() {
        return Some(run_kill_server(socket_path, invocation.args));
    }

    let (new_session_tui, native_attach) = match &resume {
        Some((resume, _)) => (
            resume.kind == ExecResumeKind::NewSession,
            resume.kind == ExecResumeKind::NativeAttach,
        ),
        None => (
            command_chain_uses_tui(&command_chain) || attach_prefix_uses_tui(&command),
            matches!(command.as_str(), "attach" | "attach-session"),
        ),
    };
    let (prepared, connection) = match resume {
        Some((resume, client)) => (Some(resume.commands), Some(client)),
        None => (None, None),
    };
    if (prepared.is_some() || start_server) && new_session_tui {
        return Some(run_tui_chain(&routing, prepared, command_chain, connection));
    }
    if native_attach {
        return Some(run_native_attach(
            &routing,
            prepared,
            command_chain,
            connection,
        ));
    }

    if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
        eprintln!("{}", error.message);
        let nested_label_new_session =
            canonical_command(&command) == "new-session" && error.kind == ErrorKind::NotFound;
        return Some(if nested_label_new_session {
            ExitCode::SUCCESS
        } else {
            exit_code_for(CliFailure::Runtime)
        });
    }
    let failure = failure.unwrap_or_else(|| {
        DaemonError::Server(ServerError::Internal(
            "daemon handed back a chain it cannot resume".to_owned(),
        ))
    });
    eprintln!("{}", format_local_command_error(socket_path, failure));
    Some(exit_code_for(CliFailure::Runtime))
}

enum LocalChain {
    Done(ExitCode),
    Resume(ExecResume, Box<CommandClient>),
    Unanswered(DaemonError),
}

fn prepare_command_client(client: &mut CommandClient) {
    client.enable_stdin();
    client.set_stderr_handler(print_command_error);
    client.set_stdout_handler(print_released_command_output);
}

fn emit_chain_outcome(outcome: &CommandOutcome) -> u8 {
    let status = print_chain_output(&outcome.stdout, raw_command_output(outcome.stdout_claim));
    print_command_error(&outcome.stderr);
    status
}

fn run_local_command_chain(
    routing: &ChainRouting<'_>,
    mut client: CommandClient,
    command_chain: &[CommandInvocation],
    progress_target: Option<&str>,
    spawned_server_id: Option<u64>,
) -> LocalChain {
    let _progress = if command_chain.len() == 1
        && let Some(pane) = progress_target
    {
        match InteractiveClient::connect_control(routing.socket_path)
            .map_err(|error| format_local_command_error(routing.socket_path, error))
            .and_then(|client| events::Progress::start(client, pane.to_owned()))
        {
            Ok(progress) => Some(progress),
            Err(error) => {
                eprintln!("agent-send: --progress: {error}");
                return LocalChain::Done(exit_code_for(CliFailure::Runtime));
            }
        }
    } else {
        None
    };
    prepare_command_client(&mut client);
    let classify: ExecClassifier<'_> = &zz_daemon::exec_resume_kind;
    let chain = ExecChain {
        commands: command_chain.to_vec(),
        spawned_server_id,
        expect_server_id: None,
        resume: Some(classify),
        prepared: false,
        last: true,
    };
    match client.exec_chain(chain, emit_chain_outcome) {
        Ok(ExecChainEnd::Ran { exit_code }) => LocalChain::Done(ExitCode::from(exit_code)),
        Ok(ExecChainEnd::Resume(resume)) => LocalChain::Resume(resume, Box::new(client)),
        Ok(ExecChainEnd::Rejected(error)) => {
            eprintln!("{}", server_error_message(&error));
            LocalChain::Done(exit_code_for(CliFailure::Server(&error)))
        }
        Ok(ExecChainEnd::Failed(error))
            if routing.command == "kill-server" && daemon_transport_failure(&error) =>
        {
            LocalChain::Done(recover_kill_server_failure(routing.socket_path, &error))
        }
        Ok(ExecChainEnd::Failed(error)) => LocalChain::Done(print_chain_failure(error, None)),
        Ok(ExecChainEnd::ServerMismatch) => LocalChain::Done(exit_code_for(CliFailure::Runtime)),
        Err(error) => LocalChain::Unanswered(error),
    }
}

fn print_chain_failure(error: DaemonError, host: Option<&str>) -> ExitCode {
    match error {
        DaemonError::CommandFailed { output, error } => {
            print_chain_output(&output, false);
            eprintln!("{}", with_host_hint(host, command_error_message(&error)));
            exit_code_for(CliFailure::Runtime)
        }
        error => {
            eprintln!("{}", with_host_hint(host, command_error_message(&error)));
            exit_code_for(CliFailure::Daemon(&error))
        }
    }
}

fn relay_declined(error: &DaemonError) -> bool {
    match error {
        DaemonError::Server(error) => error.is_command_parse(),
        DaemonError::CommandFailed { error, .. } => relay_declined(error),
        _ => false,
    }
}

fn relay_agent_send(
    socket_path: &Path,
    host: &str,
    command_chain: &mut [CommandInvocation],
) -> Option<ExitCode> {
    let [invocation] = command_chain else {
        return None;
    };
    if canonical_command(&invocation.name) != "agent-send" || std::env::var_os("ZZ_PANE").is_none()
    {
        return None;
    }
    if zz_daemon::command_stdin_sink("agent-send", &invocation.args).is_some() {
        if io::stdin().is_terminal() {
            return None;
        }
        let mut piped = Vec::new();
        io::Read::read_to_end(&mut io::stdin().lock(), &mut piped).ok()?;
        zz_daemon::append_stdin_payload(
            "agent-send",
            &mut invocation.args,
            String::from_utf8_lossy(&piped).into_owned(),
        );
    }
    let arguments = ["--relay", host]
        .into_iter()
        .map(RawText::from)
        .chain(invocation.args.iter().cloned());
    let mut client = CommandClient::connect(socket_path).ok()?;
    match client.execute_streams(CommandInvocation::new("agent-send", arguments)) {
        Ok(outcome) => {
            print_command_output(&outcome.stdout);
            print_command_error(&outcome.stderr);
            Some(ExitCode::from(outcome.exit_code))
        }
        Err(error) if relay_declined(&error) => None,
        Err(error) => {
            eprintln!("{}", command_error_message(&error));
            Some(exit_code_for(CliFailure::Daemon(&error)))
        }
    }
}

fn run_host_command_chain(
    routing: &ChainRouting<'_>,
    host: &str,
    socket_source: SocketSelectionSource,
    mut command_chain: Vec<CommandInvocation>,
    progress_target: Option<&str>,
) -> ExitCode {
    if command_chain_uses_tui(&command_chain) || attach_prefix_uses_tui(routing.command) {
        return run_tui_chain(routing, None, command_chain, None);
    }
    if matches!(routing.command, "attach" | "attach-session") {
        return run_native_attach(routing, None, command_chain, None);
    }
    if let Some(exit) = relay_agent_send(routing.socket_path, host, &mut command_chain) {
        return exit;
    }
    if let Some(error) =
        tmux_label_creation_error(routing.socket_path, socket_source, routing.start_server)
    {
        eprintln!("{}", error.message);
        let nested_label_new_session = canonical_command(routing.command) == "new-session"
            && error.kind == ErrorKind::NotFound;
        return if nested_label_new_session {
            ExitCode::SUCCESS
        } else {
            exit_code_for(CliFailure::Runtime)
        };
    }
    let mut client = match connect_host_command_client(host, chain_is_read_only(&command_chain)) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("zz: {error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let _progress = if command_chain.len() == 1
        && let Some(pane) = progress_target
    {
        match configured_host_endpoint(host)
            .and_then(|endpoint| {
                InteractiveClient::connect_control_endpoint(&endpoint)
                    .map_err(|error| host_command_error_message(host, &error))
            })
            .and_then(|client| events::Progress::start(client, pane.to_owned()))
        {
            Ok(progress) => Some(progress),
            Err(error) => {
                eprintln!("agent-send: --progress: {error}");
                return exit_code_for(CliFailure::Runtime);
            }
        }
    } else {
        None
    };
    prepare_command_client(&mut client);
    COMMAND_HOST.set(Some(host.to_owned()));
    match client.exec_chain(ExecChain::new(command_chain), emit_chain_outcome) {
        Ok(ExecChainEnd::Ran { exit_code }) => ExitCode::from(exit_code),
        Ok(ExecChainEnd::Rejected(error)) => {
            eprintln!(
                "{}",
                with_host_hint(Some(host), server_error_message(&error))
            );
            exit_code_for(CliFailure::Server(&error))
        }
        Ok(ExecChainEnd::Failed(error)) => print_chain_failure(error, Some(host)),
        Ok(ExecChainEnd::Resume(_) | ExecChainEnd::ServerMismatch) => {
            exit_code_for(CliFailure::Runtime)
        }
        Err(error) => {
            eprintln!("{}", host_command_error_message(host, &error));
            exit_code_for(CliFailure::Daemon(&error))
        }
    }
}

pub(super) fn tui_request<'a>(
    routing: &ChainRouting<'a>,
    options: &'a zz_tui::RunOptions,
    reconnect: &'a dyn Fn(
        &Path,
        bool,
        Option<zz_protocol::AttachOperation>,
    ) -> Result<InteractiveClient, DaemonError>,
) -> zz_tui::RunRequest<'a> {
    match routing.startup_options.browser_provider {
        Some(provider) => options.with_browser_provider(provider),
        None => zz_tui::RunRequest::from(options),
    }
    .with_local_reconnect(reconnect)
}

fn run_tui_chain(
    routing: &ChainRouting<'_>,
    prepared: Option<Vec<PreparedCommand>>,
    command_chain: Vec<CommandInvocation>,
    connection: Option<CommandClient>,
) -> ExitCode {
    let options = zz_tui::RunOptions {
        socket_path: routing.socket_path.to_path_buf(),
        host: routing.host.map(str::to_owned),
        session: None,
        restart_daemon: false,
        detach_others: false,
        read_only: false,
        client_flags: None,
    };
    let mux_config_files = routing.mux_config_files;
    let reconnect =
        |path: &Path, client_has_terminal, attach: Option<zz_protocol::AttachOperation>| {
            connect_terminal_surface_client_with_config(
                path,
                TerminalColorScheme::Dark,
                mux_config_files,
                client_has_terminal,
                attach.as_ref(),
            )
        };
    let request = tui_request(routing, &options, &reconnect);
    let result = match (connection, prepared) {
        (Some(client), Some(commands)) => {
            match client.into_interactive(zz_protocol::AttachOperation::Commands(commands.clone()))
            {
                Ok(initial) => zz_tui::run_connected(request, initial),
                Err(DaemonError::DaemonTooOld { .. }) => {
                    zz_tui::run_prepared_new_session(request, commands)
                }
                Err(error) => {
                    eprintln!("{}", command_error_message(&error));
                    return exit_code_for(CliFailure::Daemon(&error));
                }
            }
        }
        (_, Some(commands)) => zz_tui::run_prepared_new_session(request, commands),
        (_, None) => zz_tui::run_new_session(request, command_chain),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", tui_failure_message(routing.host, &error));
            exit_code_for(CliFailure::Runtime)
        }
    }
}

#[cfg(test)]
fn prepared_command_invocations(command: &PreparedCommand) -> Option<Cow<'_, [CommandInvocation]>> {
    if command.result != zz_protocol::PreparedCommandResult::Ready {
        return None;
    }
    if command.canonical_name.is_some() {
        return Some(Cow::Borrowed(std::slice::from_ref(&command.invocation)));
    }
    MuxEngine::command_alias_group_commands(&command.invocation)
        .ok()
        .flatten()
        .map(Cow::Owned)
}

#[cfg(test)]
pub(super) fn prepared_command_reads_stdin(command: &PreparedCommand) -> Option<CommandStdinSink> {
    prepared_command_invocations(command)?
        .iter()
        .find_map(|invocation| {
            let canonical_name = command
                .canonical_name
                .as_deref()
                .unwrap_or_else(|| canonical_command(&invocation.name));
            zz_daemon::command_stdin_sink(canonical_name, &invocation.args)
        })
}

#[cfg(test)]
pub(super) fn append_prepared_command_stdin_payload(
    command: &mut PreparedCommand,
    payload: impl Into<RawText>,
) {
    if let Some(canonical_name) = command.canonical_name.clone() {
        append_stdin_payload(&canonical_name, &mut command.invocation.args, payload);
        return;
    }
    command.invocation.set_stdin(payload);
}

pub(super) fn new_session_uses_tui(invocation: &CommandInvocation) -> bool {
    canonical_command(&invocation.name) == "new-session"
        && MuxEngine::new_session_attaches(&invocation.args).unwrap_or(false)
}

pub(super) fn attach_prefix_uses_tui(command: &str) -> bool {
    canonical_command(command) == "attach-session"
        && !matches!(command, "attach" | "attach-session")
}

#[cfg(test)]
pub(super) fn command_reads_stdin(invocation: &CommandInvocation) -> Option<CommandStdinSink> {
    zz_daemon::command_stdin_sink(canonical_command(&invocation.name), &invocation.args)
}

pub(super) fn command_chain_uses_tui(invocations: &[CommandInvocation]) -> bool {
    invocations.iter().any(new_session_uses_tui)
}

pub(super) fn split_command_chain(arguments: &[RawText]) -> Vec<CommandInvocation> {
    zz_protocol::split_command_words(arguments.iter().cloned())
        .into_iter()
        .filter_map(|words| {
            let mut words = words.into_iter();
            let name = words.next()?;
            Some(CommandInvocation::new(name, words))
        })
        .collect()
}

fn run_tmux_shell_command(
    socket_path: &Path,
    socket_source: SocketSelectionSource,
    host: Option<&str>,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
    shell_command: &str,
    login_shell: bool,
) -> ExitCode {
    let start_server = !no_start_server;
    if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
        eprintln!("{}", error.message);
        return exit_code_for(CliFailure::Runtime);
    }
    let mut client = match host.map_or_else(
        || {
            connect_command_client(socket_path, mux_config_files, start_server)
                .map_err(|error| format_local_command_error(socket_path, error))
        },
        |host| connect_host_command_client(host, false).map_err(|error| format!("zz: {error}")),
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let shell = match client.execute(CommandInvocation::new(
        "show-options",
        ["-gqv", "default-shell"],
    )) {
        Ok(shell) => shell.trim_end_matches('\n').to_owned(),
        Err(error) => {
            eprintln!(
                "{}",
                host.map_or_else(
                    || command_error_message(&error),
                    |host| host_command_error_message(host, &error)
                )
            );
            return exit_code_for(CliFailure::Daemon(&error));
        }
    };
    let mut process = Command::new(&shell);
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::process::CommandExt as _};

        let name = Path::new(&shell)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new(&shell));
        let mut argv0 = OsString::new();
        if login_shell {
            argv0.push("-");
        }
        argv0.push(name);
        process.arg0(argv0);
    }
    #[cfg(not(unix))]
    let _ = login_shell;
    match process
        .arg("-c")
        .arg(shell_command)
        .env("SHELL", &shell)
        .status_unmasked()
    {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(exit_code_for(CliFailure::Runtime), ExitCode::from),
        Err(error) => {
            eprintln!("zz: could not run {shell}: {error}");
            exit_code_for(CliFailure::Runtime)
        }
    }
}

fn is_version_command(command: &str) -> bool {
    command == "--version"
}

pub(super) fn tmux_command_starts_server(command: &str) -> bool {
    matches!(
        canonical_command(command),
        "attach-session" | "list-commands" | "list-keys" | "new-session" | "start-server"
    )
}
