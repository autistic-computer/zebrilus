use std::process::{Command, ExitCode};

use zz_daemon::agent_fork;
use zz_protocol::RawText;

const USAGE: &str = "usage: zz tools --fork-chat <claude|codex> <session> <message>";

pub(crate) fn run(arguments: &[RawText]) -> ExitCode {
    let arguments = arguments.iter().map(RawText::as_str).collect::<Vec<_>>();
    let command = match arguments.as_slice() {
        ["claude", session, message] => claude(session, message),
        ["codex", thread, item] => codex(thread, item),
        _ => Err(USAGE.to_owned()),
    };
    match command {
        Ok(mut command) => {
            let error = exec(&mut command);
            report(&format!(
                "could not start {}: {error}",
                command.get_program().display()
            ))
        }
        Err(message) => report(&message),
    }
}

#[cfg(unix)]
fn exec(command: &mut Command) -> std::io::Error {
    std::os::unix::process::CommandExt::exec(command)
}

#[cfg(not(unix))]
fn exec(command: &mut Command) -> std::io::Error {
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => error,
    }
}

fn report(message: &str) -> ExitCode {
    eprintln!("zz tools --fork-chat: {message}");
    eprintln!("Press Enter to close this pane.");
    let _ = std::io::stdin().read_line(&mut String::new());
    ExitCode::from(1)
}

fn claude(session: &str, message: &str) -> Result<Command, String> {
    let config = agent_fork::claude_config().ok_or("HOME is not set")?;
    let fork = agent_fork::claude(&config, session, Some(message))?;
    Ok(agent(&["claude", "--resume", &fork]))
}

fn agent(words: &[&str]) -> Command {
    let words = words
        .iter()
        .map(|word| (*word).to_owned())
        .collect::<Vec<_>>();
    let argv = if cfg!(unix) {
        zz_client::agent_launch::interactive_shell(&words)
    } else {
        words
    };
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command
}

fn codex(thread: &str, item: &str) -> Result<Command, String> {
    let home = agent_fork::codex_home().ok_or("HOME is not set")?;
    let fork = agent_fork::codex(&home, thread, Some(item))?;
    Ok(agent(&["codex", "resume", &fork]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_fork_resumes_through_the_users_interactive_shell() {
        let command = agent(&["claude", "--resume", "f"]);
        assert_eq!(command.get_program(), "/bin/sh");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-c", r#"exec "${SHELL:-/bin/sh}" -ic 'claude --resume f'"#]
        );
    }
}
