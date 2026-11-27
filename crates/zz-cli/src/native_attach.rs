use super::*;

const NATIVE_ATTACH_USAGE: &str = "zz: usage: zz [--host <name>] attach [--restart-daemon] [-dEr] [-c working-directory] [-f flags] [session]";

pub(super) fn run_native_attach(
    routing: &ChainRouting<'_>,
    prepared: Option<Vec<PreparedCommand>>,
    mut command_chain: Vec<CommandInvocation>,
    connection: Option<CommandClient>,
) -> ExitCode {
    let options = match parse_native_attach_arguments(command_chain[0].args.clone()) {
        Ok(options) => options,
        Err(error) => {
            return print_native_attach_argument_error(error);
        }
    };
    if options.restart_daemon && routing.host.is_some() {
        eprintln!("zz: --restart-daemon is only supported for the local daemon");
        return exit_code_for(CliFailure::Usage);
    }
    let needs_command_attach = options.no_update_environment
        || options.working_directory.is_some()
        || options.detach_others_hangup;
    let default_attach =
        !needs_command_attach && command_chain.len() == 1 && options.session.is_none();
    let attach_command = native_attach_command(&options);
    let options = zz_tui::RunOptions {
        socket_path: routing.socket_path.to_path_buf(),
        host: routing.host.map(str::to_owned),
        session: options.session,
        restart_daemon: options.restart_daemon,
        detach_others: options.detach_others,
        read_only: options.read_only,
        client_flags: options.client_flags,
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
    let result = if let (Some(client), Some(mut commands)) = (connection, prepared.clone()) {
        commands[0].invocation = attach_command;
        match client.into_interactive(zz_protocol::AttachOperation::Commands(commands.clone())) {
            Ok(initial) => zz_tui::run_connected(request, initial),
            Err(DaemonError::DaemonTooOld { .. }) => {
                zz_tui::run_prepared_new_session(request, commands)
            }
            Err(error) => {
                eprintln!("{}", command_error_message(&error));
                return exit_code_for(CliFailure::Daemon(&error));
            }
        }
    } else if command_chain.len() > 1 || needs_command_attach {
        if let Some(mut commands) = prepared {
            commands[0].invocation = attach_command;
            zz_tui::run_prepared_new_session(request, commands)
        } else {
            command_chain[0] = attach_command;
            zz_tui::run_new_session(request, command_chain)
        }
    } else {
        zz_tui::run(request)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if default_attach && error.to_string() == "can't find session: current session" {
                eprintln!("no sessions");
            } else {
                eprintln!("{}", tui_failure_message(routing.host, &error));
            }
            exit_code_for(CliFailure::Runtime)
        }
    }
}

pub(super) fn parse_native_attach_arguments(
    arguments: impl IntoIterator<Item = RawText>,
) -> Result<NativeAttachArguments, NativeAttachArgumentError> {
    let spec = catalog_command_spec("attach-session").expect("attach-session catalog");
    let mut restart_daemon = false;
    let mut detach_others = false;
    let mut detach_others_hangup = false;
    let mut no_update_environment = false;
    let mut read_only = false;
    let mut client_flags = None;
    let mut working_directory = None;
    let mut target = None;
    let mut positional = None;
    let mut options_done = false;
    let mut explicit_boundary = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if !explicit_boundary && argument == "--restart-daemon" {
            if restart_daemon {
                return Err(NativeAttachArgumentError::NativeUsage);
            }
            restart_daemon = true;
            continue;
        }
        if options_done || !argument.starts_with('-') || argument == "-" {
            if positional.is_some() {
                return Err(NativeAttachArgumentError::Usage);
            }
            positional = Some(argument);
            options_done = true;
            continue;
        }
        if argument == "--" {
            options_done = true;
            explicit_boundary = true;
            continue;
        }
        if argument.starts_with("--") {
            return Err(NativeAttachArgumentError::Command(
                ServerError::CommandParse("command attach-session: invalid flag --".to_owned()),
            ));
        }
        let options = &argument[1..];
        for (index, option) in options.char_indices() {
            let name = format!("-{option}");
            if option == '?' {
                return Err(NativeAttachArgumentError::Command(
                    ServerError::CommandParse(format!(
                        "usage: attach-session {}",
                        spec.pinned_tmux_usage()
                    )),
                ));
            }
            if !option.is_ascii_alphanumeric() {
                return Err(NativeAttachArgumentError::Command(
                    ServerError::CommandParse(format!(
                        "command attach-session: invalid flag {name}"
                    )),
                ));
            }
            match option {
                'd' => detach_others = true,
                'E' => no_update_environment = true,
                'r' => read_only = true,
                't' | 'c' | 'f' => {
                    let value_index = index + option.len_utf8();
                    let value = if value_index < options.len() {
                        options[value_index..].to_owned()
                    } else {
                        arguments
                            .next()
                            .ok_or_else(|| {
                                NativeAttachArgumentError::Command(ServerError::CommandParse(
                                    format!("command attach-session: {name} expects an argument"),
                                ))
                            })?
                            .to_string()
                    };
                    match option {
                        't' => target = Some(value),
                        'c' => working_directory = Some(value),
                        'f' => client_flags = Some(value),
                        _ => unreachable!(),
                    }
                    break;
                }
                'x' => detach_others_hangup = true,
                _ => {
                    return Err(NativeAttachArgumentError::Command(
                        ServerError::CommandParse(format!(
                            "command attach-session: unknown flag {name}"
                        )),
                    ));
                }
            }
        }
    }
    if target.is_some() && positional.is_some() {
        return Err(NativeAttachArgumentError::Usage);
    }
    Ok(NativeAttachArguments {
        restart_daemon,
        detach_others,
        detach_others_hangup,
        no_update_environment,
        read_only,
        client_flags,
        working_directory,
        session: target.or(positional.map(|value| value.to_string())),
    })
}

pub(super) fn print_native_attach_argument_error(error: NativeAttachArgumentError) -> ExitCode {
    let status = match &error {
        NativeAttachArgumentError::Usage => exit_code_for(CliFailure::TmuxUsage),
        NativeAttachArgumentError::NativeUsage => exit_code_for(CliFailure::Usage),
        NativeAttachArgumentError::Command(error) => exit_code_for(CliFailure::Server(error)),
    };
    match error {
        NativeAttachArgumentError::Usage | NativeAttachArgumentError::NativeUsage => {
            eprintln!("{NATIVE_ATTACH_USAGE}");
        }
        NativeAttachArgumentError::Command(error) => {
            eprintln!("{}", command_error_message(&DaemonError::Server(error)));
        }
    }
    status
}

pub(super) fn native_attach_command(options: &NativeAttachArguments) -> CommandInvocation {
    let mut args = Vec::new();
    if options.detach_others {
        args.push("-d".to_owned());
    }
    if options.detach_others_hangup {
        args.push("-x".to_owned());
    }
    if options.no_update_environment {
        args.push("-E".to_owned());
    }
    if options.read_only {
        args.push("-r".to_owned());
    }
    if let Some(client_flags) = &options.client_flags {
        args.extend(["-f".to_owned(), client_flags.clone()]);
    }
    if let Some(working_directory) = &options.working_directory {
        args.extend(["-c".to_owned(), working_directory.clone()]);
    }
    if let Some(session) = &options.session {
        args.extend(["-t".to_owned(), session.clone()]);
    }
    CommandInvocation::new("attach-session", args)
}
