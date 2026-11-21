mod mock_agent;

use std::{env, path::PathBuf, process::ExitCode};

use zz_daemon::Daemon;
use zz_daemon_client::{
    CommandClient, DaemonError, default_socket_path, mux_config_candidates, run_socket_proxy,
};
use zz_protocol::CommandInvocation;

fn main() -> ExitCode {
    if let Some(exit) = zz_terminal::run_pty_exec_mode() {
        return exit;
    }

    let mut args = env::args().skip(1);
    let mut socket = env::var_os("ZZ_SOCKET").map_or_else(default_socket_path, PathBuf::from);
    let mut config_files: Vec<PathBuf> = Vec::new();

    let first = loop {
        match args.next() {
            Some(flag) if flag == "-S" => {
                let Some(path) = args.next() else {
                    eprintln!("zz: missing socket path after -S");
                    return ExitCode::from(2);
                };
                socket = PathBuf::from(path);
            }
            Some(flag) if flag == "-f" => {
                let Some(path) = args.next() else {
                    eprintln!("zz: missing config path after -f");
                    return ExitCode::from(2);
                };
                config_files.push(PathBuf::from(path));
            }
            other => break other,
        }
    };

    let Some(command) = first else {
        eprintln!("usage: zz [-S SOCKET] [-f CONFIG] <daemon|proxy|mock-agent|COMMAND> [ARGS...]");
        return ExitCode::from(2);
    };

    if command == "mock-agent" {
        let provider = args.next().unwrap_or_else(|| "claude".to_owned());
        return mock_agent::run(provider);
    }

    if command == "daemon" {
        if config_files.is_empty() {
            config_files = mux_config_candidates();
        }
        return match Daemon::new(socket)
            .with_mux_config_files(config_files)
            .run_foreground()
        {
            Ok(()) | Err(DaemonError::AlreadyRunning(_)) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("zz daemon: {error}");
                ExitCode::FAILURE
            }
        };
    }

    if command == "proxy" {
        return match run_socket_proxy(&socket) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("zz proxy: {error}");
                ExitCode::FAILURE
            }
        };
    }

    let rest: Vec<String> = args.collect();
    let mut client = match CommandClient::connect(&socket) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("zz: {error}");
            return ExitCode::FAILURE;
        }
    };
    match client.execute(CommandInvocation::new(command, rest)) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("zz: {error}");
            ExitCode::FAILURE
        }
    }
}
